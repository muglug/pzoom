<?php
interface IWorker {
    public function work(): void;
}

final class Worker implements IWorker {
    public function work(): void {}
}

function f(IWorker $worker): void {
    echo get_class($worker);
}

f(new Worker());
