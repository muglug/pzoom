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
     *
     * @return TData[K]
     */
    public function __get(string $property) {
        return $this->data[$property];
    }
}