<?php
class Foo {
    /**
     * @param array<string> $arg
     * @return string
     */
    public function run($arg) : string {
        return implode("s", $arg);
    }
}

class Bar extends Foo {
    /**
     * @param array<string> $arg
     * @return string
     */
    #[ReturnTypeWillChange]
    public function run($arg) {
        return implode(" ", $arg);
    }
}
