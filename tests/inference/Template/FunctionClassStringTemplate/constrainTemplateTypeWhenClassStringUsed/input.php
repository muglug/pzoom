<?php
class GenericObjectFactory {
   /**
    * @psalm-template T
    * @psalm-param class-string<T> $type
    * @psalm-return T
    */
    public function getObject(string $type)
    {
        return 3;
    }
}
