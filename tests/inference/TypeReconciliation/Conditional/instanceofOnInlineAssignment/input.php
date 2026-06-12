<?php

abstract class Atomic2 {}

class TKeyed extends Atomic2 {
    /** @var array<string, string>|null */
    public $fallback_params;
}

abstract class U2 {
    abstract public function getSingleAtomic(): Atomic2;
}

function f(U2 $u): ?string
{
    if (($a = $u->getSingleAtomic()) instanceof TKeyed
        && $a->fallback_params === null
    ) {
        return 'keyed';
    }
    return null;
}
