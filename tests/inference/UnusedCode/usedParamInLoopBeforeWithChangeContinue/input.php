<?php
final class Foo {}

final class Bar {
    public static function build(Foo $foo) : ?self {
        echo get_class($foo);
        return new self();
    }

    public function produceFoo(): Foo {
        return new Foo();
    }
}

function takesFoo(Foo $foo): Foo {
    while (rand(0, 1)) {
        $bar = Bar::build($foo);

        if ($bar) {
            $foo = $bar->produceFoo();

            continue;
        }
    }

    return $foo;
}
