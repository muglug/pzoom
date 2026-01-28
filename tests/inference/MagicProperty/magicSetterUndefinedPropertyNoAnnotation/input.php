<?php
/** @psalm-no-seal-properties */
class A {
    public function __get(string $name): ?string {
        if ($name === "foo") {
            return "hello";
        }

        return null;
    }

    /** @param mixed $value */
    public function __set(string $name, $value): void {
    }

    public function goodSet(): void {
        $this->__set("foo", new stdClass());
    }
}
