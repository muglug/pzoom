<?php
class User {
    /**
     * @psalm-suppress NullArgument
     */
    public function give(): void{
        $model = null;
        $class = \get_class($model);
        $class::foo();
    }
}
