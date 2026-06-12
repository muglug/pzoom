<?php
    class A {
        /** @var null|int */
        public static $q = null;
    }

    class B {
        /** @psalm-assert int A::$q */
        private static function prefillQ(): void {
            A::$q = 123;
        }
        public static function getQ(): int {
            self::prefillQ();
            return A::$q;
        }
    }
?>
