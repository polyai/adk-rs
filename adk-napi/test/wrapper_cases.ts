import { strict as assert } from "node:assert";
import { test } from "node:test";
import type { AdkNapiError as AdkNapiErrorClass, Projection, pull, push } from "../dist/index.js";

type WrapperApi = {
  AdkNapiError: typeof AdkNapiErrorClass;
  pull: typeof pull;
  push: typeof push;
};

const ROOT = "project";

export function runWrapperTests({ AdkNapiError, pull, push }: WrapperApi): void {
  test("pull serializes projection objects and preserves non-ADK files", () => {
    const output = pull({
      root: ROOT,
      files: {
        "README.md": "notes\n",
        "_gen/.agent_studio_config": "ignored\n",
      },
      pullProjection: topicProjection("billing", "Remote billing"),
    });

    assert.equal(output.files["README.md"], "notes\n");
    assert.equal(output.files["_gen/.agent_studio_config"], undefined);
    assert.match(output.files["topics/billing.yaml"], /Remote billing/);
    assert.deepEqual(output.conflicts, []);
    assert.ok(
      output.changes.some(
        (change) => change.kind === "write" && change.path === "topics/billing.yaml",
      ),
    );
  });

  test("pull reports conflicts against a base projection", () => {
    const baseProjection = topicProjection("billing", "Base remote");
    const firstPull = pull({
      root: ROOT,
      files: {},
      pullProjection: baseProjection,
    });
    const localEdit =
      'name: billing\nenabled: true\nactions: ""\ncontent: "Local edit"\nexample_queries: []\n';

    const output = pull({
      root: ROOT,
      files: {
        ...firstPull.files,
        "topics/billing.yaml": localEdit,
      },
      pullProjection: topicProjection("billing", "Updated remote"),
      baseProjection,
    });

    assert.deepEqual(output.conflicts, ["topics/billing.yaml"]);
    assert.equal(output.files["topics/billing.yaml"], localEdit);
  });

  test("push returns Uint8Array command batch bytes", () => {
    const output = push({
      root: ROOT,
      files: {
        "topics/sample.yaml":
          'name: sample\nenabled: true\nactions: ""\ncontent: "hello"\nexample_queries: []\n',
      },
      projection: {},
      lastKnownSequence: 7,
      createdBy: "tester@example.com",
      currentTime: "2026-06-10T12:34:56.123Z",
      skipValidation: true,
    });

    assert.equal(output.success, true);
    assert.equal(output.message, undefined);

    const bytes = output.commandBatchBytes;
    assert.ok(bytes instanceof Uint8Array);
    assert.ok(bytes.byteLength > 0);
  });

  test("wrapper converts native errors into stable AdkNapiError objects", () => {
    assert.throws(
      () =>
        pull({
          root: ROOT,
          files: {
            "../escape.txt": "nope",
          },
          pullProjection: {},
        }),
      (error) => {
        assert.ok(error instanceof AdkNapiError);
        assert.equal(error.code, "INVALID_INPUT");
        return true;
      },
    );
  });

  test("wrapper rejects non-serializable projections before entering native code", () => {
    const projection: Record<string, unknown> = {};
    projection.self = projection;

    assert.throws(
      () =>
        pull({
          root: ROOT,
          files: {},
          pullProjection: projection,
        }),
      (error) => {
        assert.ok(error instanceof AdkNapiError);
        assert.equal(error.code, "INVALID_PROJECTION");
        return true;
      },
    );
  });
}

function topicProjection(name: string, content: string): Projection {
  return {
    knowledgeBase: {
      topics: {
        entities: {
          "topic-1": {
            id: "topic-1",
            name,
            isActive: true,
            actions: "",
            content,
            exampleQueries: [],
          },
        },
      },
    },
  };
}
