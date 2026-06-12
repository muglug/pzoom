<?php
    final class C {
        /** @var null|int */
        private static $q = null;

        /** @psalm-assert-if-true int self::$q */
        private static function prefillQ(): bool {
            if (rand(0,1)) {
                self::$q = 123;
                return true;
            }
            return false;
        }

        public static function getQ(): int {
            if (self::prefillQ()) {
                return self::$q;
            }
            return -1;
        }
    }
?>
