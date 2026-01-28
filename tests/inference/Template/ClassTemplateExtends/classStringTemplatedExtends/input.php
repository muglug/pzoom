<?php
/** @template T */
interface CrudRequest {}

/** @implements CrudRequest<string> */
class StringRequest implements CrudRequest {}

/** @template T */
interface CrudNew {
    /** @param class-string<CrudRequest<T>> $requestClass */
    public function handle(string $requestClass): void;
}

class StringNew {
    /** @param CrudNew<string> $crudNew */
    public function foo($crudNew): void {
        $crudNew->handle(StringRequest::class);
    }
}