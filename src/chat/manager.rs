use super::message::Message;

pub struct ChatManager {
    messages: Vec<Message>,
    max_history: usize,
}

impl ChatManager {
    pub fn new(max_history: usize) -> Self {
        ChatManager {
            messages: Vec::new(),
            max_history,
        }
    }
    
    pub fn add_message(&mut self, role: String, content: String) {
        self.messages.push(Message::new(role, content));
        
        if self.messages.len() > self.max_history {
            self.messages.remove(0);
        }
    }
    
    pub fn get_messages(&self) -> &Vec<Message> {
        &self.messages
    }
    
    pub fn clear(&mut self) {
        self.messages.clear();
    }
}