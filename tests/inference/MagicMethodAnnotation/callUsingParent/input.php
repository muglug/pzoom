<?php
/**
 * @method static create(array $input)
 */
class Model {
    public function __call(string $name, array $arguments) {
        /** @psalm-suppress UnsafeInstantiation */
        return new static;
    }
}

class BlahModel extends Model {
    /**
     * @param mixed $input
     * @return static
     */
    public function create($input): BlahModel
    {
        return parent::create([]);
    }
}

class FooModel extends Model {}

function consumeFoo(FooModel $a): void {}
function consumeBlah(BlahModel $a): void {}

$b = new FooModel();
consumeFoo($b->create([]));

$d = new BlahModel();
consumeBlah($d->create([]));
