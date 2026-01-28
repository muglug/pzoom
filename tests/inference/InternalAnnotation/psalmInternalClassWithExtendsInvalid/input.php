<?php
                    namespace A\B {
                        /**
                         * @psalm-internal A\B
                         */
                        class Foo { }
                    }

                    namespace A\C {
                        class Bar extends \A\B\Foo {}
                    }
