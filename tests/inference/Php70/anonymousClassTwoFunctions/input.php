<?php
interface I {}

class A
{
    /** @var ?I */
    protected $i;

    public function foo(): void
    {
        $this->i = new class implements I {};
    }

    public function foo2(): void {} // commenting this line out fixes
}
