<?php
namespace A {
    /** @return void */
    function foo() {

    }

    class Bar {

    }
}
namespace {
    A\foo();
    \A\foo();

    (new A\Bar);
}