<?php
/**
 * @template T as object
 * @mixin T
 */
class Builder {
    private $t;

    /** @param T $t */
    public function __construct(object $t) {
        $this->t = $t;
    }

    public function __call(string $method, array $parameters) {
        return $this->t->$method($parameters);
    }
}

/**
 * @method self active()
 */
class Model {
    /**
     * @return Builder<static>
     */
    public function query(): Builder {
        return new Builder($this);
    }

    public function __call(string $method, array $parameters) {
        if ($method === "active") {
            return new Model();
        }
    }
}

/** @param Builder<Model> $b */
function foo(Builder $b) : Model {
    return $b->active();
}
