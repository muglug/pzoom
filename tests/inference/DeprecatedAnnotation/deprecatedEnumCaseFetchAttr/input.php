<?php
enum Foo {
    case A;

    #[\Deprecated]
    case B;
}

Foo::B;
                
