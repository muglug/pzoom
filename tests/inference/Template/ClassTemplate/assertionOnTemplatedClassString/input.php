<?php
class TEM {
    /**
     * @template Entity as object
     * @psalm-param class-string<Entity> $type
     * @psalm-return EQB<Entity>
     */
    public function createEQB(string $type) {
        if (!class_exists($type)) {
            throw new InvalidArgumentException();
        }
        return new EQB($type);
    }
}

/**
 * @template Entity as object
 */
class EQB {
    /**
     * @psalm-var class-string<Entity>
     */
    protected $type;

    /**
     * @psalm-param class-string<Entity> $type
     */
    public function __construct(string $type) {
        $this->type = $type;
    }
}