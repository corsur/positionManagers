import {
  CloudWatchClient,
  PutMetricDataCommand,
} from "@aws-sdk/client-cloudwatch";

export class MetricHelper {
  // Private variable in JavaScript.
  #metrics;
  #cwClient;

  constructor(region) {
    this.#metrics = {};
    this.#cwClient = new CloudWatchClient({ region });
  }

  plusN(counterName, count) {
    if (counterName in this.#metrics) {
      this.#metrics[counterName] += count;
    } else {
      // Initialize for the first time.
      this.#metrics[counterName] = count;
    }
    return this;
  }

  plusOne(counterName) {
    this.plusN(counterName, 1);
    return this;
  }

  async publishMetrics(dimension, namespace) {
    const metricsData = Object.entries(this.#metrics).map(([key, value]) => {
      return {
        MetricName: key,
        Timestamp: new Date(),
        Dimensions: [
          {
            Name: "Network",
            Value: dimension,
          },
        ],
        Unit: "Count",
        Value: value,
      };
    });
    const command = {
      MetricData: metricsData,
      Namespace: namespace,
    };
    try {
      await this.#cwClient.send(new PutMetricDataCommand(command));
      console.log("Successfully published metrics to CloudWatch");
    } catch (error) {
      console.error(
        `FATAL: Failed to send metrics to CloudWatch with error ${error}.`
      );
    }
  }

  getRawMetrics() {
    return this.#metrics;
  }
}
