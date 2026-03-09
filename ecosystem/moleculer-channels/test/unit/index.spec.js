"use strict";

const { ServiceBroker } = require("moleculer-rs-client/src/compat");
const ChannelMiddleware = require("./../../").Middleware;

describe("Test service 'channelHandlerTrigger' method", () => {
	const serviceSchema = {
		name: "helper",

		channels: {
			async "helper.sum"(payload) {
				return this.sum(payload.a, payload.b);
			},

			"helper.subtract": {
				handler(payload) {
					return this.subtract(payload.a, payload.b);
				}
			}
		},

		methods: {
			sum(a, b) {
				return a + b;
			},

			subtract(a, b) {
				return a - b;
			}
		}
	};

	describe("Test service default value", () => {
		let broker = new ServiceBroker({
			logger: false,
			middlewares: [
				ChannelMiddleware({
					adapter: {
						type: "Fake"
					}
				})
			]
		});
		let service = broker.createService(serviceSchema);
		beforeAll(() => broker.start());
		afterAll(() => broker.stop());

		it("should register default 'emitLocalChannelHandler' function declaration", async () => {
			service.sum = jest.fn();
			await service.emitLocalChannelHandler("helper.sum", { a: 5, b: 5 });
			expect(service.sum).toBeCalledTimes(1);
			expect(service.sum).toBeCalledWith(5, 5);
			service.sum.mockRestore();
		});

		it("should register default 'emitLocalChannelHandler' object declaration", async () => {
			service.subtract = jest.fn();
			await service.emitLocalChannelHandler("helper.subtract", { a: 5, b: 5 });
			expect(service.subtract).toBeCalledTimes(1);
			expect(service.subtract).toBeCalledWith(5, 5);
			service.subtract.mockRestore();
		});
	});

	describe("Test service custom value", () => {
		let broker = new ServiceBroker({
			logger: false,
			middlewares: [
				ChannelMiddleware({
					channelHandlerTrigger: "myTrigger",
					adapter: {
						type: "Fake"
					}
				})
			]
		});
		let service = broker.createService(serviceSchema);
		beforeAll(() => broker.start());
		afterAll(() => broker.stop());

		it("should register with 'myTrigger'", async () => {
			service.sum = jest.fn();
			await service.myTrigger("helper.sum", { a: 5, b: 5 });
			expect(service.sum).toBeCalledTimes(1);
			expect(service.sum).toBeCalledWith(5, 5);
			service.sum.mockRestore();
		});
	});
});
