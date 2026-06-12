<?php
interface IWorker {
    public function work(): int;
}

abstract class AbstractWorker implements IWorker {
    public function work(): int {
        return 0;
    }
}

final class Worker extends AbstractWorker {
    public function work(): int {
        return 1;
    }
}

final class AnotherWorker extends AbstractWorker {}

function f(IWorker $worker): void {
    echo $worker->work();
}

f(new Worker());
f(new AnotherWorker());
