<?php
interface IWorker {
    /** @psalm-suppress PossiblyUnusedReturnValue */
    public function work(): bool;
}

final class Worker implements IWorker{
    public function work(): bool {
        return true;
    }
}

function f(IWorker $worker): void {
    $worker->work();
}

f(new Worker());
