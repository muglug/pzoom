<?php
namespace {
    class Numeric {}
}

namespace Foo {
    class Resource {}
    class Numeric {}
}

namespace Bar {
    use \Foo\Resource;
    use \Foo\Numeric;

    new \Foo\Resource();
    new \Foo\Numeric();

    new Resource();
    new Numeric();

    /**
     * @param  Resource $r
     * @param  Numeric  $n
     * @return void
     */
    function foo(Resource $r, Numeric $n) : void {}
}