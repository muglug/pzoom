<?php

namespace Psalmtest\Psalmtest;

use SomeMissingClass;

class Test
{
    public function f(): void {
        /** @var SomeMissingClass */
        $result = null;

        if ($result['errors'] === true) {}
    }
}
