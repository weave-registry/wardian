/* SampleTable: a made-up load test for when Splunk is not set up, marked as a sample everywhere it
   shows, so every part (and the USL lab, through Send to other apps) can be tried. It is never
   mistaken for Splunk's rows. Pure: it builds the table, nothing else. */
const SampleTable = Object.freeze({
  make(){
    const rows = [1, 2, 4, 8, 12, 16, 24, 32, 48, 64].map(n => {
      const x = Math.round(900 * n / (1 + 0.05 * (n - 1) + 0.0007 * n * (n - 1)));
      return [String(n), String(x), (n / x * 1000).toFixed(1)];
    });
    return {
      title: 'Sample: a load test (made up)', fields: ['concurrency', 'throughput', 'response_ms'], rows, truncated: false,
      search: '', range: '', latest: '', rangeLabel: 'Sample data, not from Splunk', at: new Date().toISOString(), seconds: 0, sample: true,
      about: {title: 'Sample load test (made up)', load: 'Requests in progress', throughput: 'Requests per second', response: 'Average response time, in milliseconds',
        method: 'Made up, to show how the table works without Splunk.'},
      units: {n: 'requests in progress', x: 'req/s', r: 'ms'}, use: {load: 'concurrency', throughput: 'throughput', response: 'response_ms'},
    };
  },
});
