<?php
interface I {
    public function work(): bool;
}

function f(I $worker): void {
    $worker->work();
}
