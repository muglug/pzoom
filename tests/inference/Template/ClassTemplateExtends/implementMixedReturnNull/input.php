<?php
/** @template T */
interface Templated {
    /** @return T */
    public function foo();
}

/**
 * @psalm-suppress MissingTemplateParam
 */
class Concrete implements Templated {
    private array $t;

    public function __construct(array $t) {
        $this->t = $t;
    }

    public function foo() {
        if (rand(0, 1)) {
            return null;
        }

        return $this->t;
    }
}
