<?php
interface I {
    public function f(float ...$rest): void;
}

class C implements I {
    /**
     * @param array<int,float> $f
     * @psalm-suppress ParamNameMismatch
     */
    public function f($f): void {}
}
