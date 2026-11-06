import { describe, expect, it } from "vitest";
import { calculateTcpPingStats, type TcpPingPacket } from "./dnsInspector";

describe("calculateTcpPingStats", () => {
  it("handles empty packet list correctly", () => {
    const stats = calculateTcpPingStats([]);
    expect(stats).toEqual({
      sent: 0,
      received: 0,
      lost: 0,
      lossRate: 0,
      min: 0,
      avg: 0,
      max: 0,
      last: 0,
      jitter: 0,
    });
  });

  it("calculates min, max, avg, jitter and 0% loss for clean packets", () => {
    const packets: TcpPingPacket[] = [
      {
        session_id: "s1",
        seq: 1,
        host: "1.1.1.1",
        port: 443,
        success: true,
        latency_ms: 10,
        timestamp: 1000,
      },
      {
        session_id: "s1",
        seq: 2,
        host: "1.1.1.1",
        port: 443,
        success: true,
        latency_ms: 20,
        timestamp: 2000,
      },
      {
        session_id: "s1",
        seq: 3,
        host: "1.1.1.1",
        port: 443,
        success: true,
        latency_ms: 15,
        timestamp: 3000,
      },
    ];

    const stats = calculateTcpPingStats(packets);
    expect(stats.sent).toBe(3);
    expect(stats.received).toBe(3);
    expect(stats.lost).toBe(0);
    expect(stats.lossRate).toBe(0);
    expect(stats.min).toBe(10);
    expect(stats.max).toBe(20);
    expect(stats.avg).toBe(15);
    expect(stats.last).toBe(15);
    // |20 - 10| = 10, |15 - 20| = 5, sum = 15, count = 2 -> jitter = 7.5
    expect(stats.jitter).toBe(7.5);
  });

  it("calculates loss rate correctly when some packets timeout or fail", () => {
    const packets: TcpPingPacket[] = [
      {
        session_id: "s1",
        seq: 1,
        host: "1.1.1.1",
        port: 443,
        success: true,
        latency_ms: 30,
        timestamp: 1000,
      },
      {
        session_id: "s1",
        seq: 2,
        host: "1.1.1.1",
        port: 443,
        success: false,
        error: "Timeout",
        timestamp: 2000,
      },
      {
        session_id: "s1",
        seq: 3,
        host: "1.1.1.1",
        port: 443,
        success: true,
        latency_ms: 40,
        timestamp: 3000,
      },
      {
        session_id: "s1",
        seq: 4,
        host: "1.1.1.1",
        port: 443,
        success: false,
        error: "Connection refused",
        timestamp: 4000,
      },
    ];

    const stats = calculateTcpPingStats(packets);
    expect(stats.sent).toBe(4);
    expect(stats.received).toBe(2);
    expect(stats.lost).toBe(2);
    expect(stats.lossRate).toBe(50);
    expect(stats.min).toBe(30);
    expect(stats.max).toBe(40);
    expect(stats.avg).toBe(35);
    expect(stats.last).toBe(40);
  });

  it("handles 100% loss correctly", () => {
    const packets: TcpPingPacket[] = [
      {
        session_id: "s1",
        seq: 1,
        host: "1.1.1.1",
        port: 443,
        success: false,
        error: "Timeout",
        timestamp: 1000,
      },
      {
        session_id: "s1",
        seq: 2,
        host: "1.1.1.1",
        port: 443,
        success: false,
        error: "Timeout",
        timestamp: 2000,
      },
    ];

    const stats = calculateTcpPingStats(packets);
    expect(stats.sent).toBe(2);
    expect(stats.received).toBe(0);
    expect(stats.lost).toBe(2);
    expect(stats.lossRate).toBe(100);
    expect(stats.min).toBe(0);
    expect(stats.avg).toBe(0);
    expect(stats.max).toBe(0);
    expect(stats.last).toBe(0);
    expect(stats.jitter).toBe(0);
  });
});
