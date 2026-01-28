<?php
/**
 * @template DATA as array<string, int|bool>
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
     * @template K as key-of<DATA>
     *
     * @param K $property
     *
     * @return DATA[K]
     */
    public function __get(string $property) {
        return $this->data[$property];
    }
}