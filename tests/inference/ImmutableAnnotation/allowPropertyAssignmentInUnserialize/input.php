<?php
/**
 * @psalm-immutable
 */
class Foo implements \Serializable {
    /** @var string */
    private $data;

    public function __construct() {
        $this->data = "Foo";
    }

    public function serialize() {
        return $this->data;
    }

    public function unserialize($data) {
        $this->data = $data;
    }

    public function getData(): string {
        return $this->data;
    }
}
