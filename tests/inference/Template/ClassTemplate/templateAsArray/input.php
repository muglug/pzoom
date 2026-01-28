<?php
/**
 * @template DATA as array<string, scalar|array|object|null>
 */
abstract class Foo {
    /**
     * @var DATA
     */
    protected $data;

    /**
     * @param DATA $data
     */
    public function __construct(array $data) {
        $this->data = $data;
    }

    /**
     * @return scalar|array|object|null
     */
    public function __get(string $property) {
        return isset($this->data[$property]) ? $this->data[$property] : null;
    }
}