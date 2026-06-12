<?php
/**
 * @template TModel of object
 * @template TDeclaringModel of object
 */
class GenericContainer {
    /** @var TModel */
    public object $model;
    /** @var TDeclaringModel */
    public object $declaringModel;
}

trait MyTrait {
    /**
     * @template T of object
     * @param class-string<T> $related
     * @return GenericContainer<T, $this>
     */
    public function withThis(string $related): GenericContainer {
        /** @var GenericContainer<T, $this> */
        return new GenericContainer();
    }
}

final class Concrete {
    use MyTrait;
}

$a = (new Concrete())->withThis(stdClass::class);
