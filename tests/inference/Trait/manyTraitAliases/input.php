<?php
trait Foo {
    public static function staticMethod():void {}
    public function nonstatic():void {}
}

Class Bar {
    use Foo {
        Foo::staticMethod as foo;
        Foo::staticMethod as foobar;
        Foo::staticMethod as fine;
        Foo::nonstatic as bad;
        Foo::nonstatic as good;
    }
}

$b = new Bar();

Bar::fine();
$b::fine();
$b->fine();

$b->good();

Bar::foo();
Bar::foobar();

$b::foo();
$b::foobar();

$b->foo();
$b->foobar();

$b->bad();
