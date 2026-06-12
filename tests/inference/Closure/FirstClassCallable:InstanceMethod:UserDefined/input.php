<?php
class Test {
    public function __construct(private readonly string $string) {
    }

    public function length(): int {
        return strlen($this->string);
    }
}
$test = new Test("test");
$closure = $test->length(...);
$length = $closure();
