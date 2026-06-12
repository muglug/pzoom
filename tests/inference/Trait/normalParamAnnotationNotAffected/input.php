<?php
class Foo {
    /**
     * @param string $bar
     * @return string
     */
    public function test(string $bar): string {
        return $bar;
    }
}

$a = (new Foo())->test("hello");
