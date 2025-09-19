import EventSource from 'eventsourcemock';
import createFetchMock from 'vitest-fetch-mock';
import {vi} from 'vitest';

const fetchMock = createFetchMock(vi);

// sets globalThis.fetch and globalThis.fetchMock to our mocked version
fetchMock.enableMocks();

window.EventSource = EventSource;