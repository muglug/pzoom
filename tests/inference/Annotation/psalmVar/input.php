<?php
class A
{
    /** @psalm-var array<int, string> */
    public $foo = [];

    public function updateFoo(): void {
        $this->foo[5] = "hello";
    }
}
