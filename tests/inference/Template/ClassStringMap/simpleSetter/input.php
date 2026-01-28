<?php
class Container {
    /** @var class-string-map<T, T> */
    public array $map = [];
    /**
     * @template U of object
     * @param class-string<U> $key
     * @param U $obj
     */
    public function set(string $key, object $obj): void {
        $this->map[$key] = $obj;
    }
}
