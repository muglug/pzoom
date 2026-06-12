<?php
namespace Lib;

/**
 * @template TResult
 */
interface Task {
    /** @return TResult */
    public function run();
}

/**
 * @psalm-import-type WorkerData from Analyzer
 * @implements Task<WorkerData>
 */
final class ShutdownTask implements Task {
    public function run() { return ['issues' => ['a' => 1], 'count' => 1]; }
}

final class Pool {
    /**
     * @template T
     * @param Task<T> $task
     * @return list<T>
     */
    public function runAll(Task $task): array { return [$task->run()]; }
}

/** @param array<string, int> $m */
function takesMap(array $m): void {}

function f(Pool $pool): void {
    foreach ($pool->runAll(new ShutdownTask) as $data) {
        takesMap($data['issues']);
    }
}

/**
 * @psalm-type WorkerData = array{issues: array<string, int>, count: int}
 */
final class Analyzer {}
