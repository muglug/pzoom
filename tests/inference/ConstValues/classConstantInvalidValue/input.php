<?php
namespace NS {
    use OtherNS\C as E;
    class C {}
    class D {};
    class F {};
    /** @psalm-param C::class|D::class|E::class $s */
    function foo(string $s) : void {}
    foo(F::class);
}

namespace OtherNS {
    class C {}
}
