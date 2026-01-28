<?php
interface X {
    public function a(): bool;
    public function b(): string;
}

function foo(?X $x): void {
    if (!($x?->a())) {
        echo $x->b();
    }
}
