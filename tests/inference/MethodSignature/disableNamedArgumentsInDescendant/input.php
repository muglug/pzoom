<?php
interface Foo {
    public function bar(string ...$_args): void;
}
final class Baz implements Foo {
    /** @no-named-arguments */
    public function bar(string ...$_args): void {}
}
                
