import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
  stages: [
    { duration: '5s', target: 20 },
    { duration: '10s', target: 50 },
    { duration: '5s', target: 0 },
  ],
  thresholds: {
    http_req_failed: ['rate<0.01'],
    http_req_duration: ['p(95)<5000'],
  },
};

const BASE = 'http://localhost:3011';

export default function () {
  const body = JSON.stringify({
    event: 'test',
    data: { num: __ITER, ts: Date.now() },
  });

  const res = http.post(BASE, body, {
    headers: {
      'Content-Type': 'application/json',
    },
  });

  check(res, {
    'status 200 or 201': (r) => r.status === 200 || r.status === 201,
    'response has id': (r) => {
      try { return JSON.parse(r.body).id !== undefined; }
      catch { return false; }
    },
  });
}
