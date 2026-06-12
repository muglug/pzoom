<?php
namespace NS;

/**
 * @psalm-type _Entry scalar|scalar[]
 */
trait T {
    /** @var array<string,_Entry> */
    public array $meta = [];
}
class C { use T; }

/**
 * @psalm-type _Eq = scalar|scalar[]
 */
class D {
    /** @var array<string,_Eq> */
    public array $meta = [];
}

function f(C $c, D $d): void {
    $c->meta['fqcn'] = "x";
    $d->meta['fqcn'] = "x";
}
