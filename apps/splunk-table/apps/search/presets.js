/* Presets: the ready-made searches under "Start from" (only the search part loads this file).
   Templates only: change the index and field names to match your data. Production traffic has no
   load level, so the traffic one measures it once a minute with Little's Law: requests in progress
   = throughput x average time per request. "minutes" says how many minutes each row averages, so
   you can see which rows rest on little data. */
const Presets = (() => {
  const LITTLE = (filter, durationField, minMinutes) => filter +
    ' | bin _time span=1m | stats count AS reqs avg(' + durationField + ') AS r_ms BY _time' +
    ' | eval x=reqs/60, n=x*r_ms/1000 | eval n=round(n*2)/2 | where n>0' +
    ' | stats count AS minutes avg(x) AS x avg(r_ms) AS r BY n | where minutes>=' + minMinutes +
    ' | sort n | table n x r minutes';
  return Object.freeze({
    loadtest: {search: 'index=loadtest | stats avg(throughput) AS x avg(latency_ms) AS r BY concurrency | table concurrency x r',
      range: '-24h', units: {n: 'users', x: 'req/s', r: 'ms'}, use: {load: 'concurrency', throughput: 'x', response: 'r'},
      note: 'Change the index and field names to match your load test, then run it.',
      about: {title: 'Load test from Splunk', load: 'Concurrent users in each step of the test', throughput: 'Requests completed per second in that step',
        response: 'Average response time in that step, in milliseconds', method: 'Each point is one load level of the test, averaged over all its samples.'}},
    traffic: {search: LITTLE('index=web sourcetype=app_requests status=200 duration_ms=*', 'duration_ms', 10),
      range: '-7d', units: {n: 'requests in progress', x: 'req/s', r: 'ms'}, use: {load: 'n', throughput: 'x', response: 'r'},
      note: 'Change the index, sourcetype and duration field to match your request logs: one event per finished request, with its time in milliseconds. The minutes column shows how many minutes each row averages.',
      about: {title: 'Requests in production (live traffic)',
        load: 'How many requests were being handled at the same time, on average, during a minute',
        throughput: 'Requests finished per second during those minutes',
        response: 'Average time to answer one request, in milliseconds',
        method: 'Not a load test: real users made this traffic. Splunk counts the finished requests in each minute and their average time. Requests in progress = requests per second × seconds per request (Little\'s Law). Minutes with the same load are averaged into one point; a load seen in fewer than 10 minutes is left out.'}},
  });
})();
