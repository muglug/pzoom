<?php
/** @psalm-template T */
interface BuilderInterface {
    /** @psalm-param T $data */
    public function create($data): Exception;
}

/** @implements BuilderInterface<string> */
class CovariantUserBuilder implements BuilderInterface {
    public function create($data): RuntimeException {
        return new RuntimeException($data);
    }
}