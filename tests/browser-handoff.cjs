// A click can finish before its asynchronous native handoff is delivered.
async function waitForHandoff(worker, expectedCount, key = 'requests') {
  return worker.evaluate(async ({ expectedCount, key }) => {
    const requests = globalThis[key];
    if (!Array.isArray(requests)) throw new Error(`Missing fixture request list: ${key}`);
    const deadline = Date.now() + 5000;
    while (requests.length < expectedCount && Date.now() < deadline) {
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    if (requests.length !== expectedCount) {
      throw new Error(`Expected ${expectedCount} delivered requests, received ${requests.length}`);
    }
    return requests;
  }, { expectedCount, key });
}
module.exports = { waitForHandoff };
