<?php

final class Logger {
    public array $entries = [];
    public function __construct() {
        $this->entries[] = 'created';
    }
}

/** @psalm-immutable */
final class Holder {
    public int $value;

    public function __construct(int $value) {
        $this->value = $value;
    }

    /** @psalm-mutation-free */
    public function describe(): string {
        // A mutation-free context allows instantiating an impure constructor
        // (only a @psalm-pure context forbids it).
        $logger = new Logger();
        return (string) count($logger->entries);
    }
}
