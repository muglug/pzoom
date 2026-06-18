<?php

abstract class Handler {
    public function handle(string $message): void {}

    public function start(int $tasks): void {}

    public function finish(int $code): void {}
}

final class LoudHandler extends Handler {
    #[Override]
    public function handle(string $message): void {
        echo $message;
    }
}

function run(Handler $handler): void {
    $handler->handle('hi');
    $handler->start(1);
    $handler->finish(0);
}

run(new LoudHandler());
