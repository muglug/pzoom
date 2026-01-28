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
     * @return key-of<DATA>
     */
    abstract public function getRandomKey() : string;
}