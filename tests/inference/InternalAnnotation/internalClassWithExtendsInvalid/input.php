<?php
                    namespace A {
                        /**
                         * @internal
                         */
                        class Foo { }
                    }

                    namespace B {
                        class Bar extends \A\Foo {}
                    }
