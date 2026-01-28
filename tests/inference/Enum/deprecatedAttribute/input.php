<?php
enum Foo {
    case A;

    #[Psalm\Deprecated]
    case B;
}

Foo::B;
                    
