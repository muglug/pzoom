<?php
class Foo {}

class Bar {
    public int $id = 3;

    public function __construct(Foo $model) {
        echo $model->id;
    }
}
