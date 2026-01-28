<?php
/**
 * @template T2
 */
class Query {
    /** @var T2 **/
    private $value;

    /**
     * @param T2 $value
     */
    public function __construct($value) {
        $this->value = $value;
    }
}

interface Temporal {
    /**
     * @template T
     * @param Query<T> $query
     */
    public function execute(Query $query) : void;
}

/**
 * @template T
 */
class Result implements Temporal {
    /** @var T **/
    private $value;

    /**
     * @param T $value
     */
    public function __construct($value) {
        $this->value = $value;
    }

    public function execute(Query $query) : void {}
}

/**
 * @param  Result<string> $result
 * @param  Query<string> $query
 */
function takesArgs(Result $result, Query $query) : void {
    $result->execute($query);
}