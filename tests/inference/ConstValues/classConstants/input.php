<?php
namespace NS {
    use OtherNS\C as E;
    class C {}
    class D {};
    /** @psalm-param C::class|D::class|E::class $s */
    function foo(string $s) : void {}
    foo(C::class);
    foo(D::class);
    foo(E::class);
    foo(\OtherNS\C::class);
}

namespace OtherNS {
    class C {}
}