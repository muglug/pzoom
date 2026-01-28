<?php
class Foo {
    const A = 1;

    #[\Deprecated]
    const B = 2;
}

Foo::B;
