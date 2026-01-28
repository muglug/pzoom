<?php
/**
 * @template T
 */
final class Value {
    /**
     * @psalm-var T
     */
    private $value;

    /**
     * @psalm-param T $value
     */
    public function __construct($value) {
        $this->value = $value;
    }

    /**
     * @psalm-return T
     */
    public function value() {
        return $this->value;
    }
}

/**
 * @template T
 * @psalm-param T $value
 * @psalm-return Value<T>
 */
function value($value): Value {
    return new Value($value);
}

/**
 * @psalm-param Value<string> $value
 */
function client($value): void {}
client(value("awdawd"));