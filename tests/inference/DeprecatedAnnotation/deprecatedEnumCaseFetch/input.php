<?php
enum Foo {
    case A;

    /** @deprecated */
    case B;
}

Foo::B;
                
