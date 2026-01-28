<?php
namespace {
    #[Attribute(Attribute::TARGET_CLASS | Attribute::TARGET_FUNCTION | Attribute::TARGET_PARAMETER)]
    class Deprecated {}
}

namespace Foo\Bar {
    function foo(#[\Deprecated] string $foo) : void {}
}