<?php
namespace Foo;

use Exception;

final class U { public function queue(): void {} }
final class Storage { public ?U $return_type = null; }

function makeU(): U { return new U(); }

function f(Storage $storage, bool $flag): void {
    try {
        $storage->return_type = makeU();
        $storage->return_type->queue();
    } catch (Exception $e) {
    }

    if ($flag && $storage->return_type) {
        echo "has type";
    }
}
