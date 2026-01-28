<?php
                    namespace A\B {
                        /**
                         * @psalm-internal A\B
                         */
                        class Foo { }
                    }

                    namespace A\B\C {
                        class Bat {
                            public function batBat() : void {
                                $a = new \A\B\Foo();
                            }
                        }
                    }
