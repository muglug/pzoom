<?php
class A {
    /** @var array<string, mixed> $bar */
    public $bar = [];

    /** @param mixed $b */
    public function foo($b) : void {
        $this->bar["a"] = $b;
    }
}
