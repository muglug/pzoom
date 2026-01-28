<?php
/**
 * @template-covariant TModel of Model
 */
class QueryBuilder {
    /**
     * @return list<TModel>
     */
    public function getInner() {
        return [];
    }
}

/**
 * @mixin QueryBuilder<static>
 */
abstract class Model {}

class FooModel extends Model {}

$f = new FooModel();
$g = $f->getInner();
