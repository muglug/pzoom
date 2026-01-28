<?php
/**
 * @psalm-immutable
 */
class Foo {
    /** @var string */
    private $data;

    public function __construct() {
        $this->data = "Foo";
    }

    public function __serialize(): array {
        return ["data" => $this->data];
    }

    /** @param array{data: string} $data */
    public function __unserialize(array $data): void {
        $this->data = $data["data"];
    }

    public function getData(): string {
        return $this->data;
    }
}
