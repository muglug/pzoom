<?php

/** @template T */
class Holder
{
    /** @param T $value */
    public function __construct(public $value) {}
}

abstract class A
{
    final public function __construct(public int $i) {}

    /** @return Holder<static> */
    public static function create(int $i): Holder
    {
        return new Holder(new static($i));
    }
}

class C extends A {}

/** @param \Closure(int):Holder<C> $_ */
function takesIntToHolder(\Closure $_): void {}

takesIntToHolder(C::create(...));
