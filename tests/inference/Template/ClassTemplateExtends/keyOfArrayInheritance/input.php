<?php
/**
 * @template DATA as array<string, int|string>
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
    abstract public function getIdProperty() : string;
}

/**
 * @template-extends Foo<array{id:int, name:string}>
 */
class FooChild extends Foo {
    public function getIdProperty() : string {
        return "id";
    }
}