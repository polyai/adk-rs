const native = require("../native.cjs") as NativeBinding;

export type FileMap = Record<string, string>;
export type Projection = unknown;

export type FileChange =
  | { kind: "write"; path: string; content: string }
  | { kind: "delete"; path: string };

export type PullInput = {
  root: string;
  files: FileMap;
  pullProjection: Projection;
  baseProjection?: Projection;
  force?: boolean;
};

export type PullOutput = {
  files: FileMap;
  changes: FileChange[];
  conflicts: string[];
};

export type PushInput = {
  root: string;
  files: FileMap;
  projection: Projection;
  lastKnownSequence: number;
  createdBy?: string;
  currentTime?: string | Date;
  force?: boolean;
  skipValidation?: boolean;
};

export type PushOutput = {
  success: boolean;
  message?: string;
  commandBatchBytes?: Uint8Array;
};

export type AdkNapiErrorCode =
  | "INVALID_INPUT"
  | "INVALID_PROJECTION"
  | "VALIDATION_FAILED"
  | "CONFLICT"
  | "COMMAND_GENERATION_FAILED"
  | "INTERNAL_ERROR";

export class AdkNapiError extends Error {
  readonly code: AdkNapiErrorCode;
  readonly details?: unknown;

  constructor(code: AdkNapiErrorCode, message: string, details?: unknown) {
    super(message);
    this.name = "AdkNapiError";
    this.code = code;
    this.details = details;
  }
}

type NativeBinding = {
  pull(input: NativePullInput): NativePullOutput;
  push(input: NativePushInput): NativePushOutput;
};

type NativePullInput = {
  root: string;
  files: FileMap;
  pullProjectionJson: string;
  baseProjectionJson?: string;
  force?: boolean;
};

type NativePullOutput = {
  files: FileMap;
  changes: NativeFileChange[];
  conflicts: string[];
};

type NativeFileChange = {
  kind: string;
  path: string;
  content?: string;
};

type NativePushInput = {
  root: string;
  files: FileMap;
  projectionJson: string;
  lastKnownSequence: number;
  createdBy?: string;
  currentTime?: string;
  force?: boolean;
  skipValidation?: boolean;
};

type NativePushOutput = {
  success: boolean;
  message?: string;
  commandBatchBytes?: Uint8Array;
};

export function pull(input: PullInput): PullOutput {
  return callNative(() => {
    const nativeInput: NativePullInput = {
      root: input.root,
      files: normalizeFileMap(input.files),
      pullProjectionJson: serializeProjection(input.pullProjection, "pullProjection"),
    };

    if (input.baseProjection !== undefined) {
      nativeInput.baseProjectionJson = serializeProjection(input.baseProjection, "baseProjection");
    }
    if (input.force !== undefined) {
      nativeInput.force = input.force;
    }

    const output = native.pull(nativeInput);

    return {
      files: normalizeFileMap(output.files),
      changes: output.changes.map(normalizeFileChange),
      conflicts: [...output.conflicts],
    };
  });
}

export function push(input: PushInput): PushOutput {
  return callNative(() => {
    const nativeInput: NativePushInput = {
      root: input.root,
      files: normalizeFileMap(input.files),
      projectionJson: serializeProjection(input.projection, "projection"),
      lastKnownSequence: normalizeLastKnownSequence(input.lastKnownSequence),
      currentTime: normalizeCurrentTime(input.currentTime),
    };

    if (input.createdBy !== undefined) {
      nativeInput.createdBy = input.createdBy;
    }
    if (input.force !== undefined) {
      nativeInput.force = input.force;
    }
    if (input.skipValidation !== undefined) {
      nativeInput.skipValidation = input.skipValidation;
    }

    const output = native.push(nativeInput);
    const result: PushOutput = {
      success: output.success,
    };

    if (output.message !== undefined) {
      result.message = output.message;
    }
    if (output.commandBatchBytes !== undefined) {
      result.commandBatchBytes = normalizeCommandBatchBytes(output.commandBatchBytes);
    }

    return result;
  });
}

function callNative<T>(callback: () => T): T {
  try {
    return callback();
  } catch (error) {
    throw normalizeNativeError(error);
  }
}

function normalizeFileMap(files: FileMap): FileMap {
  if (files === null || typeof files !== "object" || Array.isArray(files)) {
    throw new AdkNapiError("INVALID_INPUT", "files must be an object of string paths to string contents");
  }

  const normalized: FileMap = {};

  for (const [path, content] of Object.entries(files)) {
    if (typeof content !== "string") {
      throw new AdkNapiError("INVALID_INPUT", `file content must be a string for path: ${path}`);
    }
    normalized[path] = content;
  }

  return normalized;
}

function normalizeFileChange(change: NativeFileChange): FileChange {
  if (change.kind === "write") {
    if (typeof change.content !== "string") {
      throw new AdkNapiError("INTERNAL_ERROR", `write change is missing content for path: ${change.path}`);
    }
    return {
      kind: "write",
      path: change.path,
      content: change.content,
    };
  }

  if (change.kind === "delete") {
    return {
      kind: "delete",
      path: change.path,
    };
  }

  throw new AdkNapiError("INTERNAL_ERROR", `unknown file change kind: ${change.kind}`);
}

function serializeProjection(projection: Projection, fieldName: string): string {
  try {
    const serialized = JSON.stringify(projection);
    if (typeof serialized !== "string") {
      throw new TypeError(`${fieldName} must be JSON-serializable`);
    }
    return serialized;
  } catch (error) {
    throw new AdkNapiError("INVALID_PROJECTION", `${fieldName} must be JSON-serializable`, {
      cause: error,
    });
  }
}

function normalizeLastKnownSequence(value: number): number {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new AdkNapiError("INVALID_INPUT", "lastKnownSequence must be a non-negative safe integer");
  }
  return value;
}

function normalizeCurrentTime(value: string | Date | undefined): string {
  if (value === undefined) {
    return new Date().toISOString();
  }
  if (value instanceof Date) {
    const timestamp = value.getTime();
    if (!Number.isFinite(timestamp)) {
      throw new AdkNapiError("INVALID_INPUT", "currentTime must be a valid Date");
    }
    return value.toISOString();
  }
  return value;
}

function normalizeCommandBatchBytes(value: Uint8Array): Uint8Array {
  if (!(value instanceof Uint8Array)) {
    throw new AdkNapiError("INTERNAL_ERROR", "native push returned non-Uint8Array commandBatchBytes");
  }
  return value;
}

function normalizeNativeError(error: unknown): AdkNapiError {
  if (error instanceof AdkNapiError) {
    return error;
  }

  const rawMessage = error instanceof Error ? error.message : String(error);
  const match = rawMessage.match(
    /^(INVALID_INPUT|INVALID_PROJECTION|VALIDATION_FAILED|CONFLICT|COMMAND_GENERATION_FAILED|INTERNAL_ERROR):\s*(.*)$/s,
  );

  if (match) {
    return new AdkNapiError(match[1] as AdkNapiErrorCode, match[2], { cause: error });
  }

  return new AdkNapiError("INTERNAL_ERROR", rawMessage, { cause: error });
}
