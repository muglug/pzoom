<?php
                    namespace A {
                        /**
                         * @internal
                         */
                        class Foo { }
                    }

                    namespace A\B {
                        class Bat {
                            public function batBat() : void {
                                $a = new \A\Foo();
                            }
                        }
                    }
