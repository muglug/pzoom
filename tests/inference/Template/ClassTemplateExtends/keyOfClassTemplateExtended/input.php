<?php
/**
 * @template TData as array
 * @psalm-no-seal-properties
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

/** @extends DataBag<array{a: int, b: string}> */
class FooBag extends DataBag {}

$foo = new FooBag(["a" => 5, "b" => "hello"]);

$foo->a = 9;
$foo->b = "hello";

$a = $foo->a;
$b = $foo->b;
