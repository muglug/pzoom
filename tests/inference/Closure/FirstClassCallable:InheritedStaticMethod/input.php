<?php

abstract class A
{
    public function foo(int $i): string
    {
        return (string) $i;
    }
}

class C extends A {}

/** @param \Closure(int):string $_ */
function takesIntToString(\Closure $_): void {}

takesIntToString(C::foo(...));
