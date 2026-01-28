<?php
interface Foo {
    public function &foo(): int;
}

class Bar implements Foo {
    public function foo(): int {
        return 1;
    }
}
                
