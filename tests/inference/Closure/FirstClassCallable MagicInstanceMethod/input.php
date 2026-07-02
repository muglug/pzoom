<?php
/**
 * @method int length()
 */
class Test {
    public function __construct(private readonly string $string) {
    }

    public function __call(string $name, array $args): mixed {
        return match ($name) {
            "length" => strlen($this->string),
            default => throw new \Error("Undefined method"),
        };
    }
}
$test = new Test("test");
$closure = $test->length(...);
$length = $closure();
