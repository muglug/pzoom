<?php
/**
 * @template TData as array
 */
abstract class DataBag {
    /**
     * @var TData
     */
    protected $data;

    /**
     * @param TData $data
     */
    public function __construct(array $data) {
        $this->data = $data;
    }

    /**
     * @template K as key-of<TData>
     *
     * @param K $property
     * @param TData[K] $value
     */
    public function __set(string $property, $value) {
        $this->data[$property] = $value;
    }
}