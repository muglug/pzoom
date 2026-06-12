<?php
namespace Foo;

use DateTimeImmutable as DTI;

interface I { public static function m(\DateTime $e): void; }

function f(): I {
    return new class implements I {
        public static function m(\DateTime $e): void {
            echo $e->format('Y');
        }
        public function n(DTI $d): string {
            return $d->format('Y');
        }
    };
}
