<?php
                    namespace A\B {
                        /**
                         * @psalm-internal A\B
                         */
                        class Foo {
                            public function barBar(): void {
                            }
                        }

                        function getFoo(): Foo {
                            return new Foo();
                        }
                    }

                    namespace A\C {
                        class Bat {
                            public function batBat(): void {
                                \A\B\getFoo()->barBar();
                            }
                        }
                    }
