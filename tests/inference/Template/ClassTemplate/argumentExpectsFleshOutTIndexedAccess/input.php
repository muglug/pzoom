<?php
/**
 * @template TData as array
 */
abstract class Row {
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
        // validation logic would go here
        return $this->data[$property];
    }

    /**
     * @template K as key-of<TData>
     *
     * @param K $property
     * @param TData[K] $value
     */
    public function __set(string $property, $value) {
        // data updating would go here
        $this->data[$property] = $value;
    }
}

/** @extends Row<array{id: int, name: string, height: float}> */
class CharacterRow extends Row {}

$mario = new CharacterRow(["id" => 5, "name" => "Mario", "height" => 3.5]);

$mario->ame = "Luigi";
