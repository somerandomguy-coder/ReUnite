### Scenario:
- A town got hit by a severe storm with landslide, power were cut, and wifi/cellular are not available. People are still safe in their home. But food rations will soon be out. People need to communicate to the outside world to get food or emergency services.

### What is needed:
- An application, work on both phone and laptop, to connect with other peers. They can send their GPS information, and messages to other people. Using the GPS information, we can narrow down the location of each people and map their location.
- When an user open the app, they will first be put into a default network, meaning they can see other people using an ID, hash from their MAC address for privacy.
- The application will utilize mostly BLE and if possible, Wifi to communicate with other devices.
- The application can approximate the distance between each node to calculate the most efficient way/route to transfer data between devices.
- A desired feature is we can see our nearest peer devices first, using their gps and the latency on message traversal.

### What to do now:
- You have to write a thorough plan how to build this application. This will first needed to work on laptop terminal first, to test if we can connect peer to peer using our laptop. The flow must be after we initialize the app, we will direct to default network. Showing [default] as an indicator of being assigned to the default network. After that, user can create their own network using --create-network [name] and --network [name] -add [user], so they can view it privately, built by an encryption service and still be able to traverse through the nodes to reach the needed user. The option --network [name] --enable-storing should allow the user to store the messages on their own device. The network information will only live on the devices of the host and the invited user, if not other users cannot decrypt the message. To kick an user others have to vote, >= 50% of the user voted then the kicked user will be removed form the network.
- For user, the User can rename an hashed ID to a String username to see on their own device, using --rename [ID] [name]
- I need the terminal interface first, can just clone the repo or build it and run it on other devices and communicate with each other.
