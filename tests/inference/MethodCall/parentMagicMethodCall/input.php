<?php
/** @psalm-no-seal-methods */
class Model {
    /**
     * @return static
     */
    public function __call(string $method, array $args) {
        /** @psalm-suppress UnsafeInstantiation */
        return new static;
    }
}

class BlahModel extends Model {
    /**
     * @param mixed $input
     */
    public function create($input): BlahModel
    {
        return parent::create([]);
    }
}

$m = new BlahModel();
$n = $m->create([]);
